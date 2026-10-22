# ORB breakout with prior-day level confirmation

- **Thesis:** Opening-range breaks that also clear a prior-day extreme have a
  stronger auction-level confirmation and may avoid marginal mid-range breaks.
- **Inputs:** OHLCV, `opening_range_high_30`, `opening_range_low_30`,
  `prev_day_high`, `prev_day_low`, `rvol_tod_20`, `adx_14`, `atr_14`.
- **Parameters:** opening range 30/60 minutes; RVOL 1.0–1.8; ADX 18–28;
  cooldown 8–24 bars; maximum holding time 8–32 bars.
- **Decision rule:** Long only when close crosses above both the locked opening
  range high and previous-day high; short only below both corresponding lows.
  Exit on range re-entry, ATR stop, target, or time limit.
- **Expected regime:** Breakout and trend sessions with meaningful overnight
  reference levels; may undertrade on gap-heavy days.
- **Data dependencies:** Correct exchange-day/session boundary and prior-day
  reference calculations.
- **Status:** queued.
- **References:** `docs/operator/filter-dsl-catalog.md`.
