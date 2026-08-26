# Round-two winner indicator definitions

These are the exact definitions used by `run_round2.py` and the golden CSV. They are intentionally recorded without normalising them to another engine's convention.

## Input series and warmup

The Alpaca 5-minute CSV cache is loaded for each asset. The complete cached 5-minute series is resampled first:

```python
x = frame.set_index("timestamp")[["open", "high", "low", "close", "volume"]]
frame_1h = x.resample(
    "1h", label="left", closed="left"
).agg({
    "open": "first", "high": "max", "low": "min",
    "close": "last", "volume": "sum",
}).dropna().reset_index()
```

For `camp-m6`, the resampled frame is then sliced to `2025-01-01T00:00:00Z <= ts < 2025-02-01T00:00:00Z`, reset to a zero-based index, and indicators are computed on that window-local 1-hour frame. No 2024 history or extra 1-hour warmup slice is supplied to the indicator functions. The first 50 1-hour bars are skipped by the simulator (`WARMUP = 50`) for gate evaluation and entry. The first 5-minute bucket contributing to the indicator input is the `2025-01-01T00:00:00Z` bucket; the resampler consumes its available 5-minute rows in `[00:00, 01:00)`.

## EMAs

The exact pandas calls are:

```python
close = frame["close"].astype(float)
ema_9  = close.ewm(span=9,  adjust=False, min_periods=1).mean()
ema_21 = close.ewm(span=21, adjust=False, min_periods=1).mean()
ema_50 = close.ewm(span=50, adjust=False, min_periods=1).mean()
```

There is no SMA seed and no `adjust=True`: the first EMA value is the first close, followed by the recursive `adjust=False` update.

## ADX(14)

ADX is calculated directly with pandas, not through a library:

```python
prev_close = close.shift(1)
tr = pd.concat([
    high - low,
    (high - prev_close).abs(),
    (low - prev_close).abs(),
], axis=1).max(axis=1)
up = high.diff()
down = -low.diff()
plus_dm = up.where((up > down) & (up > 0), 0.0)
minus_dm = down.where((down > up) & (down > 0), 0.0)
atr = tr.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean()
plus_di = 100.0 * plus_dm.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean() / atr
minus_di = 100.0 * minus_dm.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean() / atr
dx = (100.0 * (plus_di - minus_di).abs() / (plus_di + minus_di)) \
    .replace([np.inf, -np.inf], np.nan)
adx_14 = dx.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean()
```

This is the recursive `adjust=False` Wilder-alpha form, with pandas' first-value initialisation; it is not an SMA-seeded Wilder implementation. `min_periods=14` is used for ATR, DI smoothing, and the final ADX smoothing.

## RVOL time-of-day(20)

On the 1-hour frame, `tod` is the UTC minute-of-day (`0, 60, ..., 1380`). The denominator first tries the prior 20 samples at the same UTC hour, excluding the current bar. If that same-hour history has fewer than 20 samples, it falls back to the prior 20 bars regardless of hour:

```python
tod = frame["timestamp"].dt.hour * 60 + frame["timestamp"].dt.minute
prior = volume.groupby(tod, sort=False).shift(1)
same_mean = prior.groupby(tod, sort=False).rolling(
    20, min_periods=1
).mean().reset_index(level=0, drop=True)
same_count = prior.groupby(tod, sort=False).rolling(
    20, min_periods=1
).count().reset_index(level=0, drop=True)
fallback = volume.shift(1).rolling(20, min_periods=1).mean()
denominator = same_mean.where(same_count >= 20, fallback)
rvol_tod_20 = (volume / denominator.replace(0, np.nan))
```

Since `camp-m6` starts on January 1, same-hour history is below 20 observations throughout this one-month window; therefore the fallback prior-20-bar mean is what normally supplies the denominator. A zero denominator gives NaN RVOL.

## Gate and NaN policy

The winner is 1-hour EMA(9/21/50), ADX floor 30, RVOL floor 1.3, both directions, session UTC 18:00-24:00. The exact trend predicates are:

```python
long = (ema_9 > ema_21) & (ema_21 > ema_50) & (close > ema_21) \
       & (adx_14 > 30) & (rvol_tod_20 > 1.3)
short = (ema_9 < ema_21) & (ema_21 < ema_50) & (close < ema_21) \
        & (adx_14 > 30) & (rvol_tod_20 > 1.3)
session_mask = (timestamp.dt.hour >= 18) & (timestamp.dt.hour < 24)
long_gate = long & session_mask & np.isfinite(close)
short_gate = short & session_mask & np.isfinite(close)
```

Comparisons with NaN are false, so NaN ADX/RVOL never fires a gate. The simulator starts evaluating gates at index 50 only. A gate at bar label `T` fills at the next bar label `T+1` open; it does not evaluate the next bar's gate before filling.
