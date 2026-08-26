#!/usr/bin/env python3
"""Fetch Alpaca crypto 5m bars and run the deterministic xvision grid."""
from __future__ import annotations

import itertools
import json
import math
import os
import subprocess
import time
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
import requests

try:
    from numba import njit
except ImportError:  # pragma: no cover
    def njit(fn):
        return fn

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "scratch" / "offline-opt"
BAR_DIR = OUT / "bars"
WINDOW_FILE = ROOT / ".claude" / "tmp" / "camp-windows.txt"
ASSETS = ["BTC/USD", "ETH/USD", "SOL/USD"]
FETCH_START = pd.Timestamp("2024-06-15T00:00:00Z")
FETCH_END = pd.Timestamp("2025-07-01T00:00:00Z")
COST_PER_SIDE = 0.003  # 25 bps taker + 5 bps slippage, charged on notional
RISK_PCT = 0.0075
WARMUP = 50


def windows() -> list[dict[str, str]]:
    out = []
    for line in WINDOW_FILE.read_text().splitlines():
        if line.strip():
            name, start, end = line.split("|")
            out.append({"name": name, "start": start, "end": end})
    out.extend([
        {"name": "crypto-bull-q1-2025", "start": "2025-01-01T00:00:00Z", "end": "2025-04-01T00:00:00Z"},
        {"name": "crypto-rangebound-q2-2025", "start": "2025-04-01T00:00:00Z", "end": "2025-07-01T00:00:00Z"},
        {"name": "crypto-bear-q3-2024", "start": "2024-07-01T00:00:00Z", "end": "2024-10-01T00:00:00Z"},
        {"name": "flash-crash-aug-2024", "start": "2024-08-01T00:00:00Z", "end": "2024-09-01T00:00:00Z"},
    ])
    return out


def alpaca_credentials() -> tuple[str, str]:
    # Keep credentials in memory only; never include them in logs or artifacts.
    import tomllib
    raw = subprocess.check_output(
        ["ssh", "-o", "BatchMode=yes", "root@100.120.48.1",
         "cat /mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/secrets/brokers.toml"],
        text=True,
    )
    cfg = tomllib.loads(raw)["alpaca"]
    return cfg["api_key_id"], cfg["api_secret_key"]


def fetch_asset(asset: str, key_id: str, secret: str) -> pd.DataFrame:
    BAR_DIR.mkdir(parents=True, exist_ok=True)
    safe = asset.replace("/", "_")
    path = BAR_DIR / f"{safe}_5m.csv"
    if path.exists():
        df = pd.read_csv(path, parse_dates=["timestamp"])
        df["timestamp"] = pd.to_datetime(df["timestamp"], utc=True)
        if not df.empty and df.timestamp.min() <= FETCH_START and df.timestamp.max() >= FETCH_END - pd.Timedelta(minutes=5):
            return df.sort_values("timestamp").drop_duplicates("timestamp").reset_index(drop=True)

    headers = {"APCA-API-KEY-ID": key_id, "APCA-API-SECRET-KEY": secret}
    url = "https://data.alpaca.markets/v1beta3/crypto/us/bars"
    rows: list[dict[str, Any]] = []
    token = None
    while True:
        params: dict[str, Any] = {
            "symbols": asset,
            "timeframe": "5Min",
            "start": FETCH_START.isoformat().replace("+00:00", "Z"),
            "end": FETCH_END.isoformat().replace("+00:00", "Z"),
            "limit": 10000,
            "sort": "asc",
        }
        if token:
            params["page_token"] = token
        for attempt in range(8):
            response = requests.get(url, params=params, headers=headers, timeout=60)
            if response.status_code != 429:
                break
            time.sleep(min(60.0, 2.0 ** attempt))
        response.raise_for_status()
        payload = response.json()
        bars = payload.get("bars", {}).get(asset, [])
        rows.extend(bars)
        token = payload.get("next_page_token")
        if not token:
            break
        time.sleep(0.05)
    if not rows:
        raise RuntimeError(f"Alpaca returned no bars for {asset}")
    df = pd.DataFrame(rows).rename(columns={"t": "timestamp", "o": "open", "h": "high", "l": "low", "c": "close", "v": "volume"})
    df["timestamp"] = pd.to_datetime(df["timestamp"], utc=True)
    df = df[["timestamp", "open", "high", "low", "close", "volume"]].sort_values("timestamp").drop_duplicates("timestamp").reset_index(drop=True)
    df.to_csv(path, index=False)
    return df


def indicators(df: pd.DataFrame) -> dict[str, np.ndarray]:
    close = df["close"].astype(float)
    high = df["high"].astype(float)
    low = df["low"].astype(float)
    volume = df["volume"].astype(float)
    out: dict[str, np.ndarray] = {f"ema_{n}": close.ewm(span=n, adjust=False, min_periods=1).mean().to_numpy() for n in (9, 10, 12, 21, 26, 40, 50)}
    prev_close = close.shift(1)
    tr = pd.concat([high - low, (high - prev_close).abs(), (low - prev_close).abs()], axis=1).max(axis=1)
    up = high.diff()
    down = -low.diff()
    plus_dm = up.where((up > down) & (up > 0), 0.0)
    minus_dm = down.where((down > up) & (down > 0), 0.0)
    atr = tr.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean()
    plus_di = 100.0 * plus_dm.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean() / atr
    minus_di = 100.0 * minus_dm.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean() / atr
    dx = (100.0 * (plus_di - minus_di).abs() / (plus_di + minus_di)).replace([np.inf, -np.inf], np.nan)
    out["adx_14"] = dx.ewm(alpha=1 / 14, adjust=False, min_periods=14).mean().to_numpy()
    # Same-time-of-day prior-20-day volume, with a prior-20-bar fallback.
    tod = df["timestamp"].dt.hour * 60 + df["timestamp"].dt.minute
    prior = volume.groupby(tod, sort=False).shift(1)
    same_mean = prior.groupby(tod, sort=False).rolling(20, min_periods=1).mean().reset_index(level=0, drop=True)
    same_count = prior.groupby(tod, sort=False).rolling(20, min_periods=1).count().reset_index(level=0, drop=True)
    fallback = volume.shift(1).rolling(20, min_periods=1).mean()
    denominator = same_mean.where(same_count >= 20, fallback)
    out["rvol_tod_20"] = (volume / denominator.replace(0, np.nan)).to_numpy()
    out["close"] = close.to_numpy()
    return out


@njit(cache=True)
def simulate(close: np.ndarray, opens: np.ndarray, ema_fast: np.ndarray, ema_slow: np.ndarray, ema_long: np.ndarray,
             adx: np.ndarray, rvol: np.ndarray, adx_floor: float, rvol_floor: float,
             stop_pct: float, exit_kind: int, exit_pct: float, time_exit: int, direction: int) -> float:
    """Return compounded NAV minus 1000. direction 0 long, 1 short, 2 both; exit_kind 0 TP, 1 trail."""
    nav = 1000.0
    in_pos = False
    side = 0
    entry = 0.0
    notional = 0.0
    entry_bar = -1
    peak = 0.0
    trough = 0.0
    n = close.size
    for i in range(WARMUP, n):
        # Exit on this bar close. Entry signals are evaluated at close and fill next bar open.
        if in_pos:
            p = close[i]
            exit_price = 0.0
            stopped = False
            if side == 1:
                if p <= entry * (1.0 - stop_pct):
                    exit_price = entry * (1.0 - stop_pct)
                    stopped = True
                elif exit_kind == 0 and p >= entry * (1.0 + exit_pct):
                    exit_price = p
                elif exit_kind == 1:
                    if p > peak:
                        peak = p
                    if p <= peak * (1.0 - exit_pct):
                        exit_price = peak * (1.0 - exit_pct)
                        stopped = True
                elif i - entry_bar >= time_exit:
                    exit_price = p
            else:
                if p >= entry * (1.0 + stop_pct):
                    exit_price = entry * (1.0 + stop_pct)
                    stopped = True
                elif exit_kind == 0 and p <= entry * (1.0 - exit_pct):
                    exit_price = p
                elif exit_kind == 1:
                    if p < trough:
                        trough = p
                    if p >= trough * (1.0 + exit_pct):
                        exit_price = trough * (1.0 + exit_pct)
                        stopped = True
                elif i - entry_bar >= time_exit:
                    exit_price = p
            # Time exit applies after profit/trailing checks and stop checks.
            if exit_price == 0.0 and i - entry_bar >= time_exit:
                exit_price = p
            if exit_price != 0.0:
                gross = notional * ((exit_price - entry) / entry if side == 1 else (entry - exit_price) / entry)
                nav += gross - 2.0 * notional * COST_PER_SIDE
                in_pos = False
                _ = stopped
                continue
        # Gate at close; next bar open fill. No entry on final bar.
        if not in_pos and i + 1 < n and not math.isnan(adx[i]) and not math.isnan(rvol[i]):
            long_gate = ema_fast[i] > ema_slow[i] and ema_slow[i] > ema_long[i] and close[i] > ema_slow[i] and adx[i] > adx_floor and rvol[i] > rvol_floor
            short_gate = ema_fast[i] < ema_slow[i] and ema_slow[i] < ema_long[i] and close[i] < ema_slow[i] and adx[i] > adx_floor and rvol[i] > rvol_floor
            want_long = (direction == 0 or direction == 2) and long_gate
            want_short = (direction == 1 or direction == 2) and short_gate
            if want_long or want_short:
                side = 1 if want_long else -1
                entry = opens[i + 1]
                notional = nav * RISK_PCT / stop_pct
                entry_bar = i + 1
                peak = entry
                trough = entry
                in_pos = True
    if in_pos:
        p = close[n - 1]
        gross = notional * ((p - entry) / entry if side == 1 else (entry - p) / entry)
        nav += gross - 2.0 * notional * COST_PER_SIDE
    return nav - 1000.0


def cond(ind: str, op: str, rhs: str | float) -> dict[str, Any]:
    return {"lhs": ind, "op": op, "rhs": rhs}


def filter_json(pair: tuple[int, int], adx_floor: float, rvol_floor: float, direction: int) -> dict[str, Any]:
    fast, slow = pair
    common_long = [cond(f"ema_{fast}", ">", f"ema_{slow}"), cond(f"ema_{slow}", ">", "ema_50"), cond("close", ">", f"ema_{slow}"), cond("adx_14", ">", adx_floor), cond("rvol_tod_20", ">", rvol_floor)]
    common_short = [cond(f"ema_{fast}", "<", f"ema_{slow}"), cond(f"ema_{slow}", "<", "ema_50"), cond("close", "<", f"ema_{slow}"), cond("adx_14", ">", adx_floor), cond("rvol_tod_20", ">", rvol_floor)]
    if direction == 0:
        tree: dict[str, Any] = {"all": common_long}
    elif direction == 1:
        tree = {"all": common_short}
    else:
        tree = {"any": [{"all": common_long}, {"all": common_short}]}
    return {"conditions": tree}


def strategy_contract(cfg: dict[str, Any]) -> dict[str, Any]:
    direction = cfg["direction"]
    direction_code = {"long_only": 0, "short_only": 1, "both": 2}[direction]
    dirs = ["long"] if direction == "long_only" else ["short"] if direction == "short_only" else ["long", "short"]
    policies = [{"kind": "stop_loss", "pct": cfg["stop_pct"]}]
    if cfg["exit_kind"] == "take_profit":
        policies.append({"kind": "take_profit", "pct": cfg["take_profit_pct"]})
    else:
        policies.append({"kind": "trailing_stop", "pct": cfg["trailing_pct"]})
    policies.append({"kind": "time_exit", "bars": cfg["time_exit_bars"]})
    return {
        "decision_mode": "mechanistic",
        "filter": filter_json(tuple(cfg["ema_pair"]), cfg["adx_floor"], cfg["rvol_floor"], direction_code),
        "mechanistic_config": {
            "entry_rules": [{"signal_name": "trend_gate", "direction": d} for d in dirs],
            "close_policies": policies,
        },
    }


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    wins = windows()
    key_id, secret = alpaca_credentials()
    all_data: dict[str, tuple[pd.DataFrame, dict[str, np.ndarray]]] = {}
    for asset in ASSETS:
        print(f"loading {asset}", flush=True)
        frame = fetch_asset(asset, key_id, secret)
        all_data[asset] = (frame, indicators(frame))
        print(f"  {len(frame)} bars", flush=True)

    pairs = [(9, 21), (12, 26), (10, 40)]
    configs: list[dict[str, Any]] = []
    exit_variants = [("take_profit", v) for v in (0.02, 0.03, 0.04, 0.06)] + [("trailing_stop", v) for v in (0.015, 0.02, 0.03)]
    for pair, adx_floor, rvol_floor, stop_pct, (exit_kind, exit_value), time_exit, direction in itertools.product(
        pairs, (20.0, 25.0, 30.0), (1.0, 1.3), (0.01, 0.015, 0.02, 0.03), exit_variants, (12, 48, 96), ("long_only", "short_only", "both")):
        cfg = {"ema_pair": list(pair), "adx_floor": adx_floor, "rvol_floor": rvol_floor, "stop_pct": stop_pct,
               "exit_kind": exit_kind, "exit_pct": exit_value,
               "take_profit_pct": exit_value if exit_kind == "take_profit" else None,
               "trailing_pct": exit_value if exit_kind == "trailing_stop" else None,
               "time_exit_bars": time_exit, "direction": direction}
        configs.append(cfg)
    print(f"evaluating {len(configs)} configs across {len(wins) * len(ASSETS)} asset-windows", flush=True)

    window_data: dict[tuple[str, str], tuple[np.ndarray, np.ndarray, dict[str, np.ndarray]]] = {}
    for asset, (frame, ind) in all_data.items():
        close_arr = frame.close.to_numpy(dtype=np.float64)
        open_arr = frame.open.to_numpy(dtype=np.float64)
        for w in wins:
            mask = (frame.timestamp >= pd.Timestamp(w["start"])) & (frame.timestamp < pd.Timestamp(w["end"]))
            idx = np.flatnonzero(mask.to_numpy())
            if idx.size == 0:
                window_data[(asset, w["name"])] = (np.empty(0), np.empty(0), {})
                continue
            # Indicators are computed on the broad cache, then the window starts at its own first bar.
            s, e = idx[0], idx[-1] + 1
            window_data[(asset, w["name"])] = (close_arr[s:e], open_arr[s:e], {k: v[s:e] for k, v in ind.items()})

    # Store every config's compact score and only the top five full pnl tables.
    scored: list[tuple[int, float, dict[str, Any], list[dict[str, Any]]]] = []
    direction_code = {"long_only": 0, "short_only": 1, "both": 2}
    for ci, cfg in enumerate(configs):
        profitable = 0
        total_pnl = 0.0
        table: list[dict[str, Any]] = []
        pair = cfg["ema_pair"]
        for asset in ASSETS:
            for w in wins:
                close_arr, open_arr, ind = window_data[(asset, w["name"])]
                if close_arr.size == 0:
                    pnl = 0.0
                    table.append({"window": w["name"], "asset": asset, "net_pnl": 0.0})
                    total_pnl += pnl
                    continue
                exit_kind = 0 if cfg["exit_kind"] == "take_profit" else 1
                pnl = float(simulate(close_arr, open_arr, ind[f"ema_{pair[0]}"], ind[f"ema_{pair[1]}"], ind["ema_50"], ind["adx_14"], ind["rvol_tod_20"],
                    cfg["adx_floor"], cfg["rvol_floor"], cfg["stop_pct"], exit_kind, cfg["exit_pct"], cfg["time_exit_bars"], direction_code[cfg["direction"]]))
                profitable += int(pnl > 0)
                total_pnl += pnl
                table.append({"window": w["name"], "asset": asset, "net_pnl": round(pnl, 8)})
        scored.append((profitable, total_pnl, cfg, table))
        if (ci + 1) % 500 == 0:
            print(f"  {ci + 1}/{len(configs)}", flush=True)
    scored.sort(key=lambda x: (-x[0], -x[1]))
    top = []
    for rank, (profitable, total_pnl, cfg, table) in enumerate(scored[:5], 1):
        top.append({"rank": rank, "profitable_asset_windows": profitable, "total_asset_windows": len(wins) * len(ASSETS), "total_net_pnl": round(total_pnl, 8),
                    "parameters": cfg, "strategy": strategy_contract(cfg), "per_window_pnl": table})
    result = {"method": {"windows": wins, "assets": ASSETS, "bars": "5Min", "cache": "scratch/offline-opt/bars", "grid_config_count": len(configs), "fee_bps_per_side": 25, "slippage_bps_per_side": 5, "risk_pct_per_trade": RISK_PCT, "start_nav": 1000.0, "warmup_bars": WARMUP},
              "coverage": {"profitable_asset_windows": top[0]["profitable_asset_windows"], "total_asset_windows": len(wins) * len(ASSETS)}, "top_configs": top,
              "best_config": top[0]["parameters"], "best_strategy": top[0]["strategy"], "best_per_window_pnl": top[0]["per_window_pnl"]}
    (OUT / "results.json").write_text(json.dumps(result, indent=2) + "\n")
    best = top[0]
    report = ["# Offline mechanistic optimisation", "", "## Method", "", f"Fetched Alpaca crypto 5-minute bars for BTC/USD, ETH/USD, and SOL/USD, with a local CSV cache under `scratch/offline-opt/bars/`. The broad cache is computed once per asset, so every window has indicator history before its start. The 23 campaign windows plus four canonical windows produce {len(wins)} windows and {len(wins) * len(ASSETS)} independent asset-windows.", "", "Indicators are EMA (adjust=False), Wilder ADX(14), and relative volume by UTC time-of-day over prior 20 days, falling back to prior 20 bars when a time-of-day sample is incomplete. The first 50 bars of each window are warmup. A close gate fills at the next bar open. Exit policies are checked in stop-loss, take-profit/trailing-stop, then time-exit order. The simulator compounds a $1,000 NAV independently per asset-window, sizes each position as NAV×0.0075/stop_pct, and permits one position. It charges 25 bps taker plus 5 bps slippage per side (30 bps each side on entry and exit).", "", f"The deterministic grid evaluated {len(configs)} configurations. Trailing-stop alternatives use the requested 0.015, 0.02, and 0.03 values instead of take-profit; take-profit alternatives use 0.02, 0.03, 0.04, and 0.06.", "", "## Best configuration", "", f"- Profitable asset-windows: **{best['profitable_asset_windows']}/{best['total_asset_windows']}**", f"- Total net PnL summed over asset-windows: **${best['total_net_pnl']:.2f}**", "", "```json", json.dumps(best["parameters"], indent=2), "```", "", "Transferable strategy contract:", "", "```json", json.dumps(best["strategy"], indent=2), "```", "", "## Top five", "", "| Rank | Profitable | Total net PnL | Parameters |", "|---:|---:|---:|---|"]
    for item in top:
        report.append(f"| {item['rank']} | {item['profitable_asset_windows']}/{item['total_asset_windows']} | ${item['total_net_pnl']:.2f} | `{json.dumps(item['parameters'], separators=(',', ':'))}` |")
    report += ["", "## Honest assessment", "", f"The best result covers {best['profitable_asset_windows']}/{best['total_asset_windows']} profitable asset-windows. " + ("This exceeds the requested 20-window threshold." if best["profitable_asset_windows"] > 20 else "This does not exceed 20 profitable asset-windows; no claim of broad profitability is justified."), "", "Full per-window PnL for the top five configurations is in `results.json`."]
    (OUT / "REPORT.md").write_text("\n".join(report) + "\n")
    print(f"best {best['profitable_asset_windows']}/{best['total_asset_windows']} pnl={best['total_net_pnl']:.2f}", flush=True)


if __name__ == "__main__":
    main()
