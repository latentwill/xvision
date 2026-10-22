# ORB breakout with RVOL and DMI confirmation

- **Thesis:** The first 30–60 minutes establish a short-term auction range. A
  close outside that range with unusual volume and directional movement may
  indicate continuation rather than a random wick.
- **Inputs:** OHLCV, `opening_range_high_30`, `opening_range_low_30`,
  `rvol_tod_20`, `adx_14`, `di_plus_14`, `di_minus_14`, `atr_14`.
- **Parameters:** opening range 30/60 minutes; RVOL 1.1–2.0; ADX 18–30;
  cooldown 8–24 bars; ATR stop 1.0–2.5; target 1.0–3.0R.
- **Decision rule:** Long when close crosses above the locked range high and
  RVOL/DI confirm; short symmetrically below the range low. Flat on stop,
  target, opposite breakout, or session timeout.
- **Expected regime:** Intraday trend days and high-volume expansions; weak in
  quiet range-bound sessions.
- **Data dependencies:** Session-aware 15m OHLCV and the existing filter DSL.
- **Status:** queued.
- **References:** `docs/operator/filter-dsl-catalog.md`; `docs/operator/filters.md`.
