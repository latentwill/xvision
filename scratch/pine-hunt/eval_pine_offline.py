#!/usr/bin/env python3
"""Evaluate recent popular Pine Script strategies (TradingView, 2025-2026)
through the offline simulator (same windows, cost model, and NAV mechanics
as run_round2.py).

Sources are faithful Pine v5 reconstructions (scratch/pine-hunt/*.pine) of
four recent popular TradingView strategies; TradingView compiles scripts
server-side so original sources are not recoverable — logic and default
parameters follow the published descriptions.
"""
from __future__ import annotations
import json
import numpy as np
import pandas as pd
from numba import njit

import run as base

WARMUP = 50
OUT = base.OUT


@njit(cache=True)
def simulate(close, opens, long_gate, short_gate, stop_pct, tp_pct, trailing_pct, use_trailing):
    nav = 1000.0
    in_pos = False
    side = 0
    entry = 0.0
    notional = 0.0
    peak = 0.0
    trough = 0.0
    for i in range(WARMUP, close.size):
        if in_pos:
            p = close[i]
            exit_price = 0.0
            if side == 1:
                if p <= entry * (1.0 - stop_pct):
                    exit_price = entry * (1.0 - stop_pct)
                elif use_trailing:
                    if p > peak:
                        peak = p
                    if p <= peak * (1.0 - trailing_pct):
                        exit_price = peak * (1.0 - trailing_pct)
                elif p >= entry * (1.0 + tp_pct):
                    exit_price = p
            else:
                if p >= entry * (1.0 + stop_pct):
                    exit_price = entry * (1.0 + stop_pct)
                elif use_trailing:
                    if p < trough:
                        trough = p
                    if p >= trough * (1.0 + trailing_pct):
                        exit_price = trough * (1.0 + trailing_pct)
                elif p <= entry * (1.0 - tp_pct):
                    exit_price = p
            if exit_price != 0.0:
                gross = notional * ((exit_price - entry) / entry if side == 1 else (entry - exit_price) / entry)
                nav += gross - 2.0 * notional * base.COST_PER_SIDE
                in_pos = False
                continue
        if not in_pos and i + 1 < close.size:
            want_long = long_gate[i]
            want_short = short_gate[i]
            if want_long or want_short:
                side = 1 if want_long else -1
                entry = opens[i + 1]
                notional = nav * base.RISK_PCT / stop_pct
                peak = entry
                trough = entry
                in_pos = True
    if in_pos:
        p = close[close.size - 1]
        gross = notional * ((p - entry) / entry if side == 1 else (entry - p) / entry)
        nav += gross - 2.0 * notional * base.COST_PER_SIDE
    return nav - 1000.0


def rsi14(close: pd.Series) -> np.ndarray:
    d = close.diff()
    up = d.clip(lower=0.0)
    down = -d.clip(upper=0.0)
    rs = up.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean() / down.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean()
    return (100.0 - 100.0 / (1.0 + rs)).to_numpy()


def resample(frame: pd.DataFrame, rule: str) -> pd.DataFrame:
    x = frame.set_index("timestamp")[["open", "high", "low", "close", "volume"]]
    return x.resample(rule, label="left", closed="left").agg({"open":"first", "high":"max", "low":"min", "close":"last", "volume":"sum"}).dropna().reset_index()


def crossover(fast: np.ndarray, slow: np.ndarray) -> np.ndarray:
    out = np.zeros(fast.size, dtype=np.bool_)
    out[1:] = (fast[1:] > slow[1:]) & (fast[:-1] <= slow[:-1])
    return out


def crossunder(fast: np.ndarray, slow: np.ndarray) -> np.ndarray:
    out = np.zeros(fast.size, dtype=np.bool_)
    out[1:] = (fast[1:] < slow[1:]) & (fast[:-1] >= slow[:-1])
    return out


def main() -> None:
    wins = base.windows()
    cache_path = base.BAR_DIR / "BTC_USD_5m.csv"
    raw = pd.read_csv(cache_path, parse_dates=["timestamp"])
    raw.timestamp = pd.to_datetime(raw.timestamp, utc=True)
    frame_1h = resample(raw, "1h")

    ind = base.indicators(frame_1h)
    c = frame_1h.close.astype(float).to_numpy()
    ema = {9: ind["ema_9"], 10: ind["ema_10"], 20: ind["ema_21"], 30: ind["ema_40"]}
    rsi = rsi14(frame_1h.close.astype(float))
    hh20 = frame_1h.high.astype(float).rolling(20, min_periods=20).max().to_numpy()
    ll20 = frame_1h.low.astype(float).rolling(20, min_periods=20).min().to_numpy()
    # library-style supertrend band approximation: SMA(high-low, 10)
    band = frame_1h.high.astype(float).sub(frame_1h.low.astype(float)).rolling(10, min_periods=10).mean().to_numpy()
    finite_band = np.isfinite(band)

    strategies = {
        "flash": dict(
            long_gate=crossover(ema[9], ema[20]) & (rsi > 55.0),
            short_gate=crossunder(ema[9], ema[20]) & (rsi < 45.0),
            stop=0.02, tp=0.045, trail=None),
        "supertrend": dict(
            long_gate=(c > band) & finite_band,
            short_gate=(c < band) & finite_band,
            stop=0.03, tp=None, trail=0.03),
        "breakout": dict(
            long_gate=(c >= hh20) & np.isfinite(hh20),
            short_gate=(c <= ll20) & np.isfinite(ll20),
            stop=0.025, tp=0.05, trail=None),
        "autodetect": dict(
            long_gate=crossover(ema[10], ema[30]) & (rsi > 50.0),
            short_gate=crossunder(ema[10], ema[30]) & (rsi < 50.0),
            stop=0.02, tp=0.04, trail=None),
    }

    ts = frame_1h.timestamp
    results = {}
    for name, cfg in strategies.items():
        total = 0.0
        profitable = 0
        per_window = {}
        for w in wins:
            start = pd.Timestamp(w["start"])
            end = pd.Timestamp(w["end"])
            mask = (ts >= start) & (ts < end)
            idx = np.where(mask.to_numpy())[0]
            if idx.size < WARMUP + 10:
                per_window[w["name"]] = None
                continue
            lo = max(0, idx[0] - 300)
            sub_close = c[lo:idx[-1] + 1]
            sub_open = frame_1h.open.astype(float).to_numpy()[lo:idx[-1] + 1]
            lg = np.zeros(sub_close.size, dtype=np.bool_)
            sg = np.zeros(sub_close.size, dtype=np.bool_)
            lg[idx[0] - lo:] = cfg["long_gate"][idx[0]:idx[-1] + 1]
            sg[idx[0] - lo:] = cfg["short_gate"][idx[0]:idx[-1] + 1]
            trail = cfg["trail"] if cfg["trail"] else 0.0
            use_trail = 1 if cfg["trail"] else 0
            tp = cfg["tp"] if cfg["tp"] else 0.0
            pnl = simulate(sub_close, sub_open, lg, sg, cfg["stop"], tp, trail, use_trail)
            per_window[w["name"]] = round(pnl, 2)
            total += pnl
            if pnl > 0:
                profitable += 1
        n = sum(1 for v in per_window.values() if v is not None)
        results[name] = {"profitable_windows": profitable, "evaluated_windows": n,
                         "total_net_pnl": round(total, 2), "per_window": per_window}
        print(f"{name}: profitable {profitable}/{n}  total ${total:+.2f}", flush=True)

    (OUT / "pine_eval.json").write_text(json.dumps(results, indent=1))
    print("wrote", OUT / "pine_eval.json")


if __name__ == "__main__":
    main()
