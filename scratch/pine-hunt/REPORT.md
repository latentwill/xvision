# 0x Alpha Free timeframe hunt + recent Pine strategy evaluation (2026-08-25)

## Setup

Ten rider strategies, each with the 0x Alpha Free model (`x-preview-f-free` via
`opencode-zen`) as the trader, cloned from the validated daily rider
(`01M0S9TM2NJWM07FFSZJM226WQ`). Each got:

- its own decision cadence: 15m, 30m, 1h, 2h, 4h, 8h, 12h, 1d, 3d, 1w
- a **native-timeframe filter** (the clone initially shared the daily gate; a 1d
  filter on an intraday stream only evaluates at daily closes, which would have
  made every cadence identical — each strategy now carries a filter at its own
  timeframe: `PUT /api/strategy/:id/filter`)
- a timeframe-aware system prompt

Strategy ids: `15m 01M0V6Y41XPN16ZST47Y1QY8EJ`, `30m 01M0V6Y5HGSVVGY397919R8NGE`,
`1h 01M0V6Y706ZWKZ24E59AZCCCGN`, `2h 01M0V6Y8F8426BQHKXPN4GT3JY`,
`4h 01M0V6Y9XRFC8WGKW1R916QVMP`, `8h 01M0V6YBCJADGY2G2N0QEN4FPD`,
`12h 01M0V6YCWNYC51R595A366032S`, `1d 01M0V6YEB2X6J1SAKJKF5GJX6V`,
`3d 01M0V6YFTVEND1SXAXRKVGHVEQ`, `1w 01M0V6YH9PP22E4D9CJKDSFJDS`.

Windows: four regime anchors, sized per timeframe (~130-190 bars):
Nov-2024 rally (from camp-m4), Aug-2024 flash, Apr-16-2025 range, Jun-2025
trend. Fast TFs use truncated clones (tag `tfhunt`, names `tf2-*`); 12h/1d/3d/1w
use the 90-day quarterly clones. 36 eval runs total, 3-4 parallel shards.

## Results (node backtest, % return per window)

| TF | rally | flash | range | trend | Sum | Positive |
|----|-------|-------|-------|-------|-----|----------|
| **8h** | **+14.33** | +0.11 | **+6.98** | -0.61 | **+20.80** | 3/4 |
| **2h** | **+10.56** | +0.79 | +2.13 | 0.00 | **+13.48** | 3/4 (+1 flat) |
| 1h | +2.22 | -0.11 | +1.76 | +0.10 | +3.97 | 3/4 |
| 1d | 0.00 | -0.54 | +3.54 | — | +3.00 | 1/3 |
| 12h | -2.08 | -0.19 | +4.76 | — | +2.49 | 1/3 |
| 3d | 0.00 | 0.00 | 0.00 | — | 0.00 | 0/3 (no trades) |
| 1w | 0.00 | 0.00 | 0.00 | — | 0.00 | 0/3 (no trades) |
| 4h | -0.28 | -1.71 | +2.67 | -1.23 | -0.56 | 1/4 |
| 15m | -0.83 | -0.50 | -1.89 | -0.64 | -3.85 | 0/4 |
| 30m | -0.99 | -0.83 | -1.31 | -1.45 | -4.58 | 0/4 |

## Verdict — most profitable repeatable

1. **8h cadence: +20.8% total, 3/4 windows positive, only 30 trades.** Best
   single window +14.3% (Nov 2024 rally). The 8h rider holds through multi-day
   swings and pays fees rarely.
2. **2h cadence: +13.5% total and the only TF with no losing window**
   (3 positive, 1 flat). Most consistent.
3. 1h is net positive but gives back half its edge to fees (38 trades).
4. 15m/30m lose in every window — per-trade costs (30 bps/side) exceed the
   per-bar edge of the rider.
5. 3d/1w never trade: the gate conditions (50-period MA stack) almost never
   align on 3-day/weekly bars within a 90-day window. Not viable as tested.

## Recent Pine Script strategies (TradingView 2025-2026)

TradingView compiles scripts server-side; original sources are not
recoverable from script pages. Four recent popular strategies were rebuilt as
faithful Pine v5 (`scratch/pine-hunt/*.pine`) from published logic/defaults:
The Flash-Strategy (JS_TechTrading), Advanced Supertrend (taurus79), Popular
Trend & Volatility Breakout [v6] (JohnsonForexTrader), Optimized Auto-Detect
(MA/ATR/Trend/RSI).

**Importer finding:** `strategy import-pine` maps every strategy — including
its own 11 library archetypes — to a degenerate mechanistic config
(`entry_rules: [Long, Short]`, 1% stop/target); indicator conditions are
dropped ("fuzzy expression; not wired to filter condition"). Node backtests of
imported pine are therefore every-bar entries and meaningless (verified: three
different strategies produced byte-identical PnL).

Real evaluation ran through the offline simulator (same 27 windows, 30 bps/side
costs, $1,000 NAV, `scratch/pine-hunt/eval_pine_offline.py` → `pine_eval.json`):

| Pine strategy | Profitable windows | Total net PnL |
|---|---|---|
| **Advanced Supertrend** | **17/27** | **+$109.85** |
| Optimized Auto-Detect (EMA 10/40 + RSI 50) | 13/27 | -$125.27 |
| Trend & Volatility Breakout (Donchian-20) | 11/27 | -$139.93 |
| The Flash-Strategy (EMA 9/21 + RSI) | 8/27 | -$509.55 |

Only the supertrend archetype is net-positive; the crossover composites lose
to fees, same as the fast rider cadences.

## Cross-check

The 0x Alpha Free rider at 8h/2h (+20.8%/+13.5% over 4 regime windows) beats
every pine strategy evaluated (+$110 best, ≈ +11% on $1,000×27 windows but
spread thin) and the offline round2 grid best (22/27 windows but negative net
PnL at BTC-only book before the trailing-stop patch).

## Caveats

- Fast-TF windows are truncated regime starts (2-45 days), slow TFs use full
  quarters — cross-TF totals are not bar-for-bar comparable; per-window
  selection is regime-matched, not date-matched.
- 2h/trend window: gate never opened (0 trades, flat) — legitimate.
- Free tier flakiness required retries (~10-15% of runs); all 36 windows
  completed. Proxy (`zen-strip-proxy`, port 8917) must stay up for any rerun.
