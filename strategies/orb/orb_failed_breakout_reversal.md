# Failed ORB breakout reversal

- **Thesis:** A breakout that cannot hold outside the opening range can trap
  momentum traders. Returning inside the range with weakening volume may create
  a mean-reversion opportunity toward the range midpoint.
- **Inputs:** OHLCV, `opening_range_high_30`, `opening_range_low_30`,
  `opening_range_mid_30`, `rvol_tod_20`, `atr_14`, `rsi_14`.
- **Parameters:** opening range 30/60 minutes; re-entry window 1–4 bars;
  RVOL failure ceiling 0.8–1.2; RSI confirmation 35–65; target midpoint or
  1.0–2.0R; cooldown 8–24 bars.
- **Decision rule:** After a prior close outside the range, enter short when
  price crosses back below the range high, or long when it crosses back above
  the range low. Exit at midpoint/target, ATR stop, opposite range break, or
  session timeout.
- **Expected regime:** False-break and range-bound sessions; vulnerable to
  strong trend days where the re-entry is brief.
- **Data dependencies:** A stateful prior-break condition; if unavailable in a
  single filter, use a strategy agent or explicit event state.
- **Status:** queued.
- **References:** `docs/operator/filter-dsl-catalog.md`.
