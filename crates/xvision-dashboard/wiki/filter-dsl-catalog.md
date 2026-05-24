# Filter DSL catalog

This is the deterministic inline filter catalog used by `strategy.filter`
and `xvn strategy set-filter`. Use these exact tokens when authoring
filters from chat rail, the CLI, or JSON.

## Required filter shape

Inline filters are JSON objects. `xvn strategy set-filter` also accepts
`{ "filter": { ... } }` and fills `strategy_id` from the positional
strategy id. Required author-facing fields are:

- `display_name`
- `asset_scope` - exactly one symbol in v1, for example `["BTC/USD"]`
- `timeframe`
- `conditions`

Useful optional fields:

- `cooldown_bars`
- `max_wakeups_per_day`
- `wake_when_in_position`
- `description`

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

Filters serialize back to the canonical tokens above. The parser also
accepts common inbound aliases (`gt`, `above`, `lt`, `below`, `gte`,
`lte`, `eq`, `equals`, `crosses_over`, `crosses_under`) so chat rail
repairs can normalize user/model phrasing without another failed tool
call. `crosses_above` and `crosses_below` never accept numeric
right-hand sides.

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
- `donchian_upper_<period>` - 2 to 200
- `donchian_middle_<period>` - 2 to 200
- `donchian_lower_<period>` - 2 to 200

Momentum and oscillators:

- `rsi_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `roc_<period>` - 2 to 200
- `stoch_k_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `stoch_d_<period>` - 2 to 200, numeric thresholds must be 0 to 100
- `cci_<period>` - 2 to 200
- `mfi_<period>` - 2 to 200, numeric thresholds must be 0 to 100

Volatility and bands:

- `atr_<period>` - 2 to 200
- `atr_pct_<period>` - 2 to 200, numeric thresholds must be greater than 0
- `bb_upper_<period>` - 2 to 200
- `bb_middle_<period>` - 2 to 200
- `bb_lower_<period>` - 2 to 200
- `bb_width_<period>` - 2 to 200
- `bb_pct_b_<period>` - 2 to 200

MACD:

- `macd_line`, `macd`, `macd_12_26_9`, and `macd_line_12_26_9`
- `macd_signal` and `macd_signal_12_26_9`
- `macd_hist`, `macd_histogram`, `macd_hist_12_26_9`, and `macd_histogram_12_26_9`

MACD uses the standard 12/26/9 EMA components.

Volume-aware:

- `vwap_<period>` - 2 to 200
- `volume_sma_<period>` - 2 to 200
- `obv`

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

## See also

- [Firing Conditions](/docs?slug=firing-conditions)
- [CLI Reference](/docs?slug=cli-reference)
