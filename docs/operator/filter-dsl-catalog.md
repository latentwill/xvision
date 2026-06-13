# Filter DSL catalog

This is the deterministic inline filter catalog used by
`strategy.filter` and `xvn strategy set-filter`. Use these exact tokens
when authoring filters from chat rail, the CLI, or JSON.

## Required filter shape

Inline filters are JSON objects. `xvn strategy set-filter` also accepts
`{ "filter": { ... } }` and fills `strategy_id` from the positional
strategy id. Required author-facing fields are:

- `display_name`
- `asset_scope` - exactly one symbol in v1, for example `["BTC/USD"]`
- `timeframe`
- `conditions`

> **`asset_scope` MUST be a JSON array** (U10). Write `["BTC/USD"]`, not a bare
> string `"BTC/USD"`. A bare string is rejected by the parser even though it
> "reads" like a single symbol. The array form holds exactly one symbol in v1.
> To see the canonical filter shape and accepted tokens from the CLI, run
> `xvn strategy filter-catalog --json`.

The runtime defaults are `status: "draft"`, `scan_cadence: "bar_close"`,
`cooldown_bars: 0`, `max_wakeups_per_day: null`,
`wake_when_in_position: "on_invalidation_or_target_only"`, and
`agent_context_template: "compact_trade_context_v1"` when those fields
are omitted by higher-level authoring surfaces.

`wake_when_in_position` controls whether the trader LLM is invoked while a
position is open:

- `on_invalidation_or_target_only` (default) — wake only on a fresh trip (the
  bar the condition tree first becomes true again), so a new
  invalidation/target signal still lets the trader close. The sustained-true
  bars in between are suppressed, so a position is NOT re-evaluated on every
  bar. This is the cost-safe default.
- `always` — wake on every bar the tree is true while holding (the first true
  bar AND every sustained-true bar). Expensive: a level operator that stays
  true drives a trader-LLM call on every in-position bar. Opt-in only.
- `never` — never wake while holding; exits rely entirely on the deterministic
  `risk.stop_loss_atr_multiple`.

> **Gotcha — `on_invalidation_or_target_only` suppresses re-fires while in a
> position (U11).** With the default `on_invalidation_or_target_only`, the
> filter will **NOT re-fire while a position is open in that asset** (it wakes
> only on a fresh trip of the condition tree). If your entry condition stays
> true after the entry and you have no distinct exit signal, the trader is
> never re-invoked to close — so a whole backtest can complete with only **1–2
> decisions**. Use this default only when you also have a reliable exit signal
> (a target/invalidation condition or `risk.stop_loss_atr_multiple`). When this
> setting gates a would-be re-fire, the eval now emits a `filter_blocked` event
> with `reason = position_open`, so a sparse-decision run is visible in the run
> detail's filter events. If you need re-evaluation on every in-position bar,
> set `wake_when_in_position: "always"` (expensive — one trader-LLM call per
> in-position bar).

Filters may also include optional LLM trigger metadata:

- `fire.reason` - short machine-readable trigger reason
- `fire.priority` - 0.0 to 1.0 relative priority for downstream surfaces
- `fire.tags` - compact category labels
- `fire.context` - indicator tokens to attach to the trigger payload

`fire` metadata does not make a filter pass or fail. It only controls
the compact reason/context bundle exposed to traces and trader briefings
when the boolean gate is active.

## Operators

| Operator | DSL token | Operand contract |
| --- | --- | --- |
| Greater than | `>` | indicator lhs, indicator or numeric rhs |
| Less than | `<` | indicator lhs, indicator or numeric rhs |
| Greater or equal | `>=` | indicator lhs, indicator or numeric rhs |
| Less or equal | `<=` | indicator lhs, indicator or numeric rhs |
| Equal | `==` | indicator lhs, indicator or numeric rhs |
| Crosses above | `crosses_above` | indicator lhs, indicator rhs |
| Crosses below | `crosses_below` | indicator lhs, indicator rhs |
| Between inclusive | `between` | indicator lhs, two-number range rhs |
| Above for N bars | `above_for_<bars>` | indicator lhs, indicator or numeric rhs |
| Below for N bars | `below_for_<bars>` | indicator lhs, indicator or numeric rhs |
| Crossed above recently | `crossed_above_<bars>` | indicator lhs, indicator rhs |
| Crossed below recently | `crossed_below_<bars>` | indicator lhs, indicator rhs |
| Slope greater than | `slope_gt_<bars>` | indicator lhs, numeric rhs |
| Slope less than | `slope_lt_<bars>` | indicator lhs, numeric rhs |
| Z-score greater than | `zscore_gt_<period>` | indicator lhs, numeric rhs |
| Z-score less than | `zscore_lt_<period>` | indicator lhs, numeric rhs |
| Within percent | `within_pct_<pct>` | indicator lhs, indicator or numeric rhs |

Filters serialize back to the canonical tokens above. The parser also
accepts common inbound aliases (`gt`, `above`, `lt`, `below`, `gte`,
`lte`, `eq`, `equals`, `crosses_over`, `crosses_under`) so chat rail
repairs can normalize user/model phrasing without another failed tool
call. `crosses_above` and `crosses_below` never accept numeric
right-hand sides. For a numeric threshold crossing, compare against a
bounded oscillator with `>` or `<` and use `cooldown_bars` to limit
repeats.

Parameterized operator tokens encode the parameter directly. For
example, `above_for_3` means the lhs must be above the rhs on the current
bar and the previous two evaluated bars. `crossed_above_5` remains true
when the cross occurred on the current bar or within the previous four
evaluated bars. `slope_gt_4` compares the lhs change versus four bars
ago against the numeric rhs. `zscore_gt_20` computes the lhs z-score over
the current bar plus the previous 19 evaluated values. `within_pct_1.5`
tests whether lhs is within 1.5% of rhs.

## Indicator tokens

Price and volume primitives:

- `open`
- `high`
- `low`
- `close`
- `volume`

Moving averages and trend:

- `sma_<period>` - 2 to 500
- `ema_<period>` - 2 to 500
- `wma_<period>` - 2 to 500
- `adx_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `di_plus_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `di_minus_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `donchian_upper_<period>` - 2 to 200
- `donchian_middle_<period>` - 2 to 200
- `donchian_lower_<period>` - 2 to 200
- `highest_<period>` - 2 to 200, rolling N-bar high
- `lowest_<period>` - 2 to 200, rolling N-bar low
- `opening_range_high_<minutes>` - 2 to 200, current session opening-range high after the window locks
- `opening_range_low_<minutes>` - 2 to 200, current session opening-range low after the window locks
- `opening_range_mid_<minutes>` - 2 to 200, midpoint of opening-range high/low after the window locks

Ichimoku:

- `tenkan` - 9-bar conversion line
- `kijun` - 26-bar base line
- `senkou_a` - current cloud span A, unshifted for filter evaluation
- `senkou_b` - current cloud span B, unshifted for filter evaluation
- `chikou` - close from 26 bars ago
- `cloud_top` - max of `senkou_a` and `senkou_b`
- `cloud_bottom` - min of `senkou_a` and `senkou_b`
- `cloud_thickness` - absolute cloud span distance

Momentum and oscillators:

- `rsi_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `roc_<period>` - 2 to 200, percent rate of change
- `stoch_k_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `stoch_d_<period>` - 2 to 200, 3-bar SMA of stochastic K, numeric thresholds must be 0 to 100
- `stoch_rsi_<period>` - alias for `stoch_rsi_k_<period>`
- `stoch_rsi_k_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `stoch_rsi_d_<period>` - 2 to 200, 3-bar SMA of StochRSI K, numeric thresholds must be 0 to 100
- `cci_<period>` - 2 to 200
- `mfi_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `williams_r_<period>` - 2 to 200, numeric thresholds must be -100 to 0

Volatility and bands:

- `atr_<period>` - 2 to 200
- `atr_pct_<period>` - 2 to 200, numeric thresholds must be greater than 0
- `bb_upper_<period>` - 2 to 200, 2 standard deviation Bollinger upper band
- `bb_middle_<period>` - 2 to 200, Bollinger middle SMA
- `bb_lower_<period>` - 2 to 200, 2 standard deviation Bollinger lower band
- `bb_width_<period>` - 2 to 200, `(upper - lower) / middle`
- `bb_pct_b_<period>` - 2 to 200, `(close - lower) / (upper - lower)`
- `keltner_upper_<period>` - 2 to 200, EMA middle plus 2 ATR
- `keltner_middle_<period>` - 2 to 200, EMA middle
- `keltner_lower_<period>` - 2 to 200, EMA middle minus 2 ATR

MACD:

- `macd_line`, `macd`, `macd_12_26_9`, and `macd_line_12_26_9`
- `macd_signal` and `macd_signal_12_26_9`
- `macd_hist`, `macd_histogram`, `macd_hist_12_26_9`, and `macd_histogram_12_26_9`

MACD uses the standard 12/26/9 EMA components.

Volume-aware:

- `vwap_<period>` - 2 to 200, rolling typical-price VWAP
- `volume_sma_<period>` - 2 to 200
- `rvol_<period>` - current volume divided by same-time-of-day average when timestamps are available, otherwise rolling volume average
- `rvol_tod_<period>` - explicit same-time-of-day relative volume token; same fallback behavior as `rvol_<period>`
- `volume_zscore_<period>` - current volume normalized by trailing rolling volume mean/stddev
- `obv` - cumulative On-Balance Volume

Pine Script catalog parity (WU5 — native, no external crate):

- `hma_<period>` - 2 to 500, Hull Moving Average (`WMA(2*WMA(n/2) - WMA(n), sqrt(n))`); lower lag than SMA/EMA
- `vwma_<period>` - 2 to 500, Volume-Weighted Moving Average (`sum(close*volume, n) / sum(volume, n)`)
- `supertrend_<atr_period>_<mult×10>` - ATR-based trailing stop/trend indicator; the token encodes ATR period and multiplier scaled by 10. Example: `supertrend_10_30` = ATR period 10, multiplier 3.0. Valid ranges: atr_period 2–200, mult×10 1–200. Emits the active band level; compare with `close` to determine trend direction.
- `pivot_high_<left>_<right>` - highest high over a lookback window of `left + right + 1` bars centred on the candidate bar. Emits the last confirmed pivot-high level. Valid: left 1–100, right 1–100.
- `pivot_low_<left>_<right>` - lowest low over the same window. Emits the last confirmed pivot-low level.

Session and reference levels:

- `prev_day_open`
- `prev_day_high`
- `prev_day_low`
- `prev_day_close`
- `prev_week_high`
- `prev_week_low`
- `prev_week_close`
- `prev_month_open`
- `prev_month_high`
- `prev_month_low`
- `prev_month_close`
- `premarket_high`
- `premarket_low`
- `gap_pct` - current session open versus previous day close, percent
- `gap_up` - 1 when `gap_pct > 0`, else 0
- `gap_down` - 1 when `gap_pct < 0`, else 0

## Examples

EMA cross with a 16-bar cooldown:

```json
{
  "display_name": "BTC 15m EMA cross",
  "asset_scope": ["BTC/USD"],
  "timeframe": "15m",
  "conditions": {
    "any": [
      { "lhs": "ema_12", "op": "crosses_above", "rhs": "ema_26" },
      { "lhs": "ema_12", "op": "crosses_below", "rhs": "ema_26" }
    ]
  },
  "cooldown_bars": 16
}
```

LLM trigger context for an opening-range breakout:

```json
{
  "display_name": "LLM trend breakout fire",
  "asset_scope": ["BTC/USD"],
  "timeframe": "15m",
  "conditions": {
    "all": [
      { "lhs": "adx_14", "op": ">", "rhs": 25 },
      { "lhs": "di_plus_14", "op": "above_for_3", "rhs": "di_minus_14" },
      { "lhs": "close", "op": "crossed_above_3", "rhs": "opening_range_high_30" },
      { "lhs": "rvol_tod_20", "op": ">", "rhs": 1.5 }
    ]
  },
  "fire": {
    "reason": "trend_breakout",
    "priority": 0.85,
    "tags": ["trend", "breakout", "volume"],
    "context": [
      "close",
      "opening_range_high_30",
      "adx_14",
      "di_plus_14",
      "di_minus_14",
      "rvol_tod_20",
      "volume_zscore_20"
    ]
  },
  "cooldown_bars": 16
}
```

MACD and Bollinger mean-reversion context:

```json
{
  "display_name": "MACD BB pullback",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "conditions": {
    "all": [
      { "lhs": "bb_pct_b_20", "op": "<", "rhs": 0.2 },
      { "lhs": "macd_hist", "op": ">", "rhs": 0 },
      { "lhs": "rsi_14", "op": "between", "rhs": [30, 70] }
    ]
  },
  "cooldown_bars": 8
}
```

Breakout with trend and liquidity confirmation:

```json
{
  "display_name": "Donchian volume breakout",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "conditions": {
    "all": [
      { "lhs": "close", "op": "crosses_above", "rhs": "donchian_upper_20" },
      { "lhs": "ema_50", "op": ">", "rhs": "ema_200" },
      { "lhs": "volume", "op": ">", "rhs": "volume_sma_20" },
      { "lhs": "atr_pct_14", "op": ">", "rhs": 0.6 }
    ]
  },
  "cooldown_bars": 12
}
```

ADX-gated trend persistence:

```json
{
  "display_name": "ADX trend persistence",
  "asset_scope": ["BTC/USD"],
  "timeframe": "15m",
  "conditions": {
    "all": [
      { "lhs": "adx_14", "op": ">", "rhs": 25 },
      { "lhs": "di_plus_14", "op": "above_for_3", "rhs": "di_minus_14" },
      { "lhs": "close", "op": "crossed_above_5", "rhs": "keltner_upper_20" },
      { "lhs": "rvol_20", "op": ">", "rhs": 1.5 }
    ]
  },
  "cooldown_bars": 16
}
```

Ichimoku cloud confirmation:

```json
{
  "display_name": "Ichimoku cloud confirmation",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "conditions": {
    "all": [
      { "lhs": "close", "op": ">", "rhs": "cloud_top" },
      { "lhs": "tenkan", "op": ">", "rhs": "kijun" },
      { "lhs": "cloud_thickness", "op": "slope_gt_3", "rhs": 0 }
    ]
  },
  "cooldown_bars": 8
}
```

## Research basis

This catalog covers the common strategy components surfaced in
Freqtrade, Hummingbot, TA-Lib, and Backtrader references: moving
averages, MACD, RSI, Stochastic/StochRSI, ADX/DI, Ichimoku, Bollinger
Bands, Keltner Channels, ATR, Donchian/rolling high-low, Williams %R,
volume flow, RVOL/time-of-day RVOL, opening-range/session references,
and VWAP. Relevant public references:

- https://www.freqtrade.io/en/stable/strategy-customization/
- https://technical.freqtrade.io/1.4.3/
- https://ta-lib.org/
- https://backtrader.readthedocs.io/en/latest/api/backtrader.indicators.html
- https://hummingbot.org/blog/directional-trading-with-macd-and-bollinger-bands/
