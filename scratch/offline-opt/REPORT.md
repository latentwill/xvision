# Offline mechanistic optimisation

## Method

Fetched Alpaca crypto 5-minute bars for BTC/USD, ETH/USD, and SOL/USD, with a local CSV cache under `scratch/offline-opt/bars/`. The broad cache is computed once per asset, so every window has indicator history before its start. The 23 campaign windows plus four canonical windows produce 27 windows and 81 independent asset-windows.

Indicators are EMA (adjust=False), Wilder ADX(14), and relative volume by UTC time-of-day over prior 20 days, falling back to prior 20 bars when a time-of-day sample is incomplete. The first 50 bars of each window are warmup. A close gate fills at the next bar open. Exit policies are checked in stop-loss, take-profit/trailing-stop, then time-exit order. The simulator compounds a $1,000 NAV independently per asset-window, sizes each position as NAV×0.0075/stop_pct, and permits one position. It charges 25 bps taker plus 5 bps slippage per side (30 bps each side on entry and exit).

The deterministic grid evaluated 4536 configurations. Trailing-stop alternatives use the requested 0.015, 0.02, and 0.03 values instead of take-profit; take-profit alternatives use 0.02, 0.03, 0.04, and 0.06.

## Best configuration

- Profitable asset-windows: **13/81**
- Total net PnL summed over asset-windows: **$-2659.75**

```json
{
  "ema_pair": [
    9,
    21
  ],
  "adx_floor": 25.0,
  "rvol_floor": 1.3,
  "stop_pct": 0.03,
  "exit_kind": "trailing_stop",
  "exit_pct": 0.02,
  "take_profit_pct": null,
  "trailing_pct": 0.02,
  "time_exit_bars": 96,
  "direction": "short_only"
}
```

Transferable strategy contract:

```json
{
  "decision_mode": "mechanistic",
  "filter": {
    "conditions": {
      "all": [
        {
          "lhs": "ema_9",
          "op": "<",
          "rhs": "ema_21"
        },
        {
          "lhs": "ema_21",
          "op": "<",
          "rhs": "ema_50"
        },
        {
          "lhs": "close",
          "op": "<",
          "rhs": "ema_21"
        },
        {
          "lhs": "adx_14",
          "op": ">",
          "rhs": 25.0
        },
        {
          "lhs": "rvol_tod_20",
          "op": ">",
          "rhs": 1.3
        }
      ]
    }
  },
  "mechanistic_config": {
    "entry_rules": [
      {
        "signal_name": "trend_gate",
        "direction": "short"
      }
    ],
    "close_policies": [
      {
        "kind": "stop_loss",
        "pct": 0.03
      },
      {
        "kind": "trailing_stop",
        "pct": 0.02
      },
      {
        "kind": "time_exit",
        "bars": 96
      }
    ]
  }
}
```

## Top five

| Rank | Profitable | Total net PnL | Parameters |
|---:|---:|---:|---|
| 1 | 13/81 | $-2659.75 | `{"ema_pair":[9,21],"adx_floor":25.0,"rvol_floor":1.3,"stop_pct":0.03,"exit_kind":"trailing_stop","exit_pct":0.02,"take_profit_pct":null,"trailing_pct":0.02,"time_exit_bars":96,"direction":"short_only"}` |
| 2 | 13/81 | $-4039.47 | `{"ema_pair":[9,21],"adx_floor":25.0,"rvol_floor":1.3,"stop_pct":0.02,"exit_kind":"trailing_stop","exit_pct":0.02,"take_profit_pct":null,"trailing_pct":0.02,"time_exit_bars":96,"direction":"short_only"}` |
| 3 | 13/81 | $-5578.21 | `{"ema_pair":[9,21],"adx_floor":25.0,"rvol_floor":1.3,"stop_pct":0.015,"exit_kind":"take_profit","exit_pct":0.04,"take_profit_pct":0.04,"trailing_pct":null,"time_exit_bars":96,"direction":"short_only"}` |
| 4 | 12/81 | $-2147.86 | `{"ema_pair":[12,26],"adx_floor":30.0,"rvol_floor":1.0,"stop_pct":0.03,"exit_kind":"take_profit","exit_pct":0.04,"take_profit_pct":0.04,"trailing_pct":null,"time_exit_bars":96,"direction":"short_only"}` |
| 5 | 12/81 | $-2150.39 | `{"ema_pair":[9,21],"adx_floor":30.0,"rvol_floor":1.0,"stop_pct":0.03,"exit_kind":"take_profit","exit_pct":0.04,"take_profit_pct":0.04,"trailing_pct":null,"time_exit_bars":96,"direction":"short_only"}` |

## Honest assessment

The best result covers 13/81 profitable asset-windows. This does not exceed 20 profitable asset-windows; no claim of broad profitability is justified.

Full per-window PnL for the top five configurations is in `results.json`.

## Round two: lower frequency and mean reversion

Round two evaluated 24624 deterministic configurations at 15-minute and 1-hour resolution. It tested the original trend gate and a Bollinger(20, 2) plus RSI(14) mean-reversion gate, each in four UTC six-hour entry sessions. Results are scored at strategy level: the three asset PnLs are summed per window, then a window counts profitable when that sum is positive.

Best round-two result: **22/27 windows** profitable, total net PnL **$1530.13**.

- Best `trend`: **22/27** windows, total net PnL **$1530.13**. Parameters: `{"resolution":"1h","family":"trend","session_utc":"18-24","direction":"both","stop_pct":0.02,"exit_kind":"trailing_stop","exit_pct":0.03,"time_exit_bars":96,"ema_pair":[9,21],"adx_floor":30.0,"rvol_floor":1.3}`
- Best `mean_reversion`: **17/27** windows, total net PnL **$201.16**. Parameters: `{"resolution":"1h","family":"mean_reversion","session_utc":"12-18","direction":"both","stop_pct":0.03,"exit_kind":"trailing_stop","exit_pct":0.03,"time_exit_bars":96,"ema_pair":null,"adx_floor":null,"rvol_floor":null}`

```json
{
  "resolution": "1h",
  "family": "trend",
  "session_utc": "18-24",
  "direction": "both",
  "stop_pct": 0.02,
  "exit_kind": "trailing_stop",
  "exit_pct": 0.03,
  "time_exit_bars": 96,
  "ema_pair": [
    9,
    21
  ],
  "adx_floor": 30.0,
  "rvol_floor": 1.3
}
```

The full round-two top-five and per-asset-window table is in `results.json` under `round2`; `best_by_family` records the strongest configuration for each family.
