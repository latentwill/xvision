# Plan: Funding Rate Enrichment for LLM Discrimination

**Goal:** Give the trader LLM a signal the filter cannot evaluate, so it has a genuine basis to veto gate fires. Funding rate / premium index directly measures market leverage sentiment — orthogonal to all current OHLCV indicators.

**Why this matters:** Current strategy has ~8% LLM skip rate (rubber-stamps every gate fire). Optimizer can't improve a model that never discriminates. This is the prerequisite before re-running optimization.

---

## Data Sources (two-path)

| Path | Source | Endpoint | Auth | History |
|---|---|---|---|---|
| Backtest | Binance data.binance.vision S3 | `premiumIndexKlines/BTCUSDT/1h/BTCUSDT-1h-{date}.zip` | None | 2020→present |
| Live / paper | Orderly Network REST | `GET /v1/public/funding_rate/PERP_BTC_USDC` | None | Current + ~1yr |

**Why premium index for backtest instead of funding rate:**
Binance's actual funding rate API is geo-blocked from US/SG datacenters. The premium index klines (perp price minus spot price, hourly OHLC) are the direct input to the funding rate formula and carry the same signal: positive = longs overpaying for leverage, negative = shorts overpaying. Available on S3 with no restrictions.

**Symbol mapping:**
- xvn `BTC/USDT` → Binance S3 `BTCUSDT` → Orderly `PERP_BTC_USDC`

---

## Implementation Steps

### 1. Bar data fetch: download + cache premium index / funding rate

**Backtest path (`xvn bars fetch` or scenario build):**
- Add a `FundingRateFetcher` that downloads Binance S3 premium index CSVs for the requested symbol/date range
- Parse columns: `open_time, open, high, low, close` (close = premium at end of hour)
- Store as a separate SQLite table: `funding_rate_cache(symbol, ts_ms, premium_index_close, source)`
- One row per 1h bar; fetched once and reused across scenarios

**Live path (`eval run --mode paper` or live):**
- At decision time, query `https://api.orderly.org/v1/public/funding_rate/PERP_{ASSET}_USDC`
- Returns `last_funding_rate` (most recent 8h settlement) and `est_funding_rate` (current estimate)
- Cache for the session to avoid per-cycle HTTP calls

### 2. Alignment: join to 15m bars

Funding rate / premium index changes every 1h (premium) or 8h (funding). Decision bars are 15m.

Strategy: **forward-fill** — each 15m bar inherits the most recent preceding funding rate snapshot.

```
bar_ts=09:15  →  funding from 09:00 premium close
bar_ts=09:30  →  funding from 09:00 premium close
bar_ts=09:45  →  funding from 09:00 premium close
bar_ts=10:00  →  funding from 10:00 premium close  (new snapshot)
```

This is accurate for backtest (the 09:00 close IS the known value at 09:15+).

### 3. Filter DSL: new indicator tokens

Add to the filter catalog:

| Token | Source | Example condition |
|---|---|---|
| `funding_rate` | 8h settlement value (Orderly live, derived from premium for backtest) | `funding_rate > 0.0001` |
| `premium_index_close` | hourly premium index close | `premium_index_close > 0.001` |
| `premium_index_1h_avg` | rolling 1h mean of premium | optional, smoother signal |

Useful filter conditions for the mean-reversion strategy:
- `funding_rate > 0.0001` — longs overleveraged, fade has tailwind (pay short carry)
- `funding_rate < 0` — shorts overleveraged, longs earn carry — extra conviction for long fade
- `abs(funding_rate) < 0.00005` — neutral funding, mechanical fade only

### 4. Indicator snapshot: inject into trader briefing

Add to the `IndicatorSnapshot` struct and the trader's context block:

```
funding_rate: 0.000165          # 8h rate (annualized: ~18%)
funding_rate_regime: "elevated" # bucketed: negative / neutral / elevated / extreme
premium_index_1h: -0.00081      # perp trading 0.08% below spot right now
```

The regime bucket is the important addition — the LLM doesn't need raw basis-point arithmetic, it needs a label it can reason about: "elevated funding + price below VWAP = strong fade conviction" vs "neutral funding + price below VWAP = mechanical setup only."

### 5. Trader prompt update

Add a decision axis to the trader system prompt:

```
Funding context: consider funding_rate when sizing conviction.
- Elevated positive funding (>0.01%/8h): longs overpaying, fade has carry support → higher conviction
- Negative funding: shorts paying longs, mean-reversion less certain → reduce conviction or skip
- Extreme funding (>0.05%/8h): potential squeeze risk despite technical setup → flat
```

This is what the optimizer can then tune — the exact thresholds and the skip criteria.

---

## Verification Gate (before re-running optimizer)

After adding funding rate context, run fino1 on 3 scenarios and check:

1. **Skip rate > 25%** — model chooses `flat` on at least 1 in 4 gate fires
2. **Funding-correlated decisions** — `flat` decisions cluster at neutral/negative funding; `long_open` at elevated positive funding
3. **Conviction variance** — conviction scores spread across 0.4–0.9 range (not clustered at 0.65–0.85)

If skip rate is still <15% after prompt update, the prompt isn't surfacing the funding context effectively — revise before optimizing.

---

## Out of Scope (this wave)

- OI (`sum_open_interest`) from Binance metrics S3 — available same path, similar alignment. Add in a follow-on after funding rate is validated.
- Long/short ratio (`count_long_short_ratio`) — useful but noisier; defer.
- Nansen / on-chain flows — higher value but requires API key and more complex data pipeline.
