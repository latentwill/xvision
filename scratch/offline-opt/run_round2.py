#!/usr/bin/env python3
"""Round two: lower-frequency trend and BB/RSI mean reversion with UTC sessions."""
from __future__ import annotations
import itertools, json
import argparse
from pathlib import Path
import numpy as np
import pandas as pd
from numba import njit

import run as base

OUT = base.OUT
WARMUP = 50
DIRECTIONS = {"long_only": 0, "short_only": 1, "both": 2}
SESSIONS = [(0, 6), (6, 12), (12, 18), (18, 24)]

@njit(cache=True)
def simulate_gate(close, opens, long_gate, short_gate, stop_pct, exit_kind, exit_pct, time_exit, direction):
    nav = 1000.0
    in_pos = False
    side = 0
    entry = 0.0
    notional = 0.0
    entry_bar = -1
    peak = 0.0
    trough = 0.0
    for i in range(WARMUP, close.size):
        if in_pos:
            p = close[i]
            exit_price = 0.0
            if side == 1:
                if p <= entry * (1.0 - stop_pct):
                    exit_price = entry * (1.0 - stop_pct)
                elif exit_kind == 0 and p >= entry * (1.0 + exit_pct):
                    exit_price = p
                elif exit_kind == 1:
                    if p > peak: peak = p
                    if p <= peak * (1.0 - exit_pct): exit_price = peak * (1.0 - exit_pct)
            else:
                if p >= entry * (1.0 + stop_pct):
                    exit_price = entry * (1.0 + stop_pct)
                elif exit_kind == 0 and p <= entry * (1.0 - exit_pct):
                    exit_price = p
                elif exit_kind == 1:
                    if p < trough: trough = p
                    if p >= trough * (1.0 + exit_pct): exit_price = trough * (1.0 + exit_pct)
            if exit_price == 0.0 and i - entry_bar >= time_exit: exit_price = p
            if exit_price != 0.0:
                gross = notional * ((exit_price-entry)/entry if side == 1 else (entry-exit_price)/entry)
                nav += gross - 2.0 * notional * base.COST_PER_SIDE
                in_pos = False
                continue
        if not in_pos and i + 1 < close.size:
            want_long = (direction == 0 or direction == 2) and long_gate[i]
            want_short = (direction == 1 or direction == 2) and short_gate[i]
            if want_long or want_short:
                side = 1 if want_long else -1
                entry = opens[i + 1]
                notional = nav * base.RISK_PCT / stop_pct
                entry_bar = i + 1
                peak = entry
                trough = entry
                in_pos = True
    if in_pos:
        p = close[close.size - 1]
        gross = notional * ((p-entry)/entry if side == 1 else (entry-p)/entry)
        nav += gross - 2.0 * notional * base.COST_PER_SIDE
    return nav - 1000.0


def rsi14(close: pd.Series) -> np.ndarray:
    d = close.diff()
    up = d.clip(lower=0.0)
    down = -d.clip(upper=0.0)
    rs = up.ewm(alpha=1/14, adjust=False, min_periods=14).mean() / down.ewm(alpha=1/14, adjust=False, min_periods=14).mean()
    return (100.0 - 100.0 / (1.0 + rs)).to_numpy()


def resample(frame: pd.DataFrame, rule: str) -> pd.DataFrame:
    x = frame.set_index("timestamp")[["open", "high", "low", "close", "volume"]]
    return x.resample(rule, label="left", closed="left").agg({"open":"first", "high":"max", "low":"min", "close":"last", "volume":"sum"}).dropna().reset_index()


def make_gates(frame: pd.DataFrame, family: str, pair: tuple[int,int], adx_floor: float, rvol_floor: float, session: tuple[int,int], ind: dict[str,np.ndarray] | None = None) -> tuple[np.ndarray,np.ndarray]:
    if ind is None: ind = base.indicators(frame)
    c = frame.close.astype(float)
    if family == "trend":
        long = (ind[f"ema_{pair[0]}"] > ind[f"ema_{pair[1]}"]) & (ind[f"ema_{pair[1]}"] > ind["ema_50"]) & (ind["close"] > ind[f"ema_{pair[1]}"]) & (ind["adx_14"] > adx_floor) & (ind["rvol_tod_20"] > rvol_floor)
        short = (ind[f"ema_{pair[0]}"] < ind[f"ema_{pair[1]}"]) & (ind[f"ema_{pair[1]}"] < ind["ema_50"]) & (ind["close"] < ind[f"ema_{pair[1]}"]) & (ind["adx_14"] > adx_floor) & (ind["rvol_tod_20"] > rvol_floor)
    else:
        mid = c.rolling(20, min_periods=20).mean()
        sd = c.rolling(20, min_periods=20).std(ddof=0)
        rsi = rsi14(c)
        long = (c.to_numpy() < (mid - 2.0*sd).to_numpy()) & (rsi < 30.0)
        short = (c.to_numpy() > (mid + 2.0*sd).to_numpy()) & (rsi > 70.0)
    h = frame.timestamp.dt.hour.to_numpy()
    session_mask = (h >= session[0]) & (h < session[1])
    return np.asarray(long & session_mask & np.isfinite(ind["close"])), np.asarray(short & session_mask & np.isfinite(ind["close"]))


def contract(cfg: dict) -> dict:
    if cfg["family"] == "trend":
        fast, slow = cfg["ema_pair"]
        long = [{"lhs":f"ema_{fast}","op":">","rhs":f"ema_{slow}"},{"lhs":f"ema_{slow}","op":">","rhs":"ema_50"},{"lhs":"close","op":">","rhs":f"ema_{slow}"},{"lhs":"adx_14","op":">","rhs":cfg["adx_floor"]},{"lhs":"rvol_tod_20","op":">","rhs":cfg["rvol_floor"]}]
        short = [{"lhs":f"ema_{fast}","op":"<","rhs":f"ema_{slow}"},{"lhs":f"ema_{slow}","op":"<","rhs":"ema_50"},{"lhs":"close","op":"<","rhs":f"ema_{slow}"},{"lhs":"adx_14","op":">","rhs":cfg["adx_floor"]},{"lhs":"rvol_tod_20","op":">","rhs":cfg["rvol_floor"]}]
    else:
        long = [{"lhs":"close","op":"<","rhs":"bb_lower_20_2"},{"lhs":"rsi_14","op":"<","rhs":30.0}]
        short = [{"lhs":"close","op":">","rhs":"bb_upper_20_2"},{"lhs":"rsi_14","op":">","rhs":70.0}]
    d = cfg["direction"]
    tree = {"all":long} if d == "long_only" else {"all":short} if d == "short_only" else {"any":[{"all":long},{"all":short}]}
    dirs = ["long"] if d == "long_only" else ["short"] if d == "short_only" else ["long","short"]
    policy = [{"kind":"stop_loss","pct":cfg["stop_pct"]}, {"kind":"take_profit" if cfg["exit_kind"] == "take_profit" else "trailing_stop", "pct":cfg["exit_pct"]}, {"kind":"time_exit","bars":cfg["time_exit_bars"]}]
    return {"decision_mode":"mechanistic","filter":{"conditions":tree},"mechanistic_config":{"entry_rules":[{"signal_name":"round2_gate","direction":x} for x in dirs],"close_policies":policy}}

def dump_trades() -> None:
    winner = json.loads((OUT / "results.json").read_text())["round2"]["best_config"]
    asset = "BTC/USD"
    raw = pd.read_csv(base.BAR_DIR / "BTC_USD_5m.csv", parse_dates=["timestamp"])
    raw.timestamp = pd.to_datetime(raw.timestamp, utc=True)
    frame = resample(raw, "1h")
    for window_name, limit in (("camp-m6", 10), ("camp-m7", 5)):
        w = next(x for x in base.windows() if x["name"] == window_name)
        mask = (frame.timestamp >= pd.Timestamp(w["start"])) & (frame.timestamp < pd.Timestamp(w["end"]))
        ix = np.flatnonzero(mask.to_numpy())
        if not ix.size:
            continue
        sub = frame.iloc[ix[0]:ix[-1] + 1].reset_index(drop=True)
        ind = base.indicators(sub)
        session = tuple(map(int, winner["session_utc"].split("-")))
        long_gate, short_gate = make_gates(sub, "trend", tuple(winner["ema_pair"]), winner["adx_floor"], winner["rvol_floor"], session, ind)
        trades = []
        in_pos = False
        side = 0
        entry_i = -1
        signal_i = -1
        peak = trough = 0.0
        for i in range(WARMUP, len(sub)):
            p = float(sub.close.iloc[i])
            if in_pos:
                exit_px = None
                if side == 1:
                    if p <= entry_px * (1 - winner["stop_pct"]):
                        exit_px = entry_px * (1 - winner["stop_pct"])
                    else:
                        if p > peak: peak = p
                        if p <= peak * (1 - winner["exit_pct"]):
                            exit_px = peak * (1 - winner["exit_pct"])
                else:
                    if p >= entry_px * (1 + winner["stop_pct"]):
                        exit_px = entry_px * (1 + winner["stop_pct"])
                    else:
                        if p < trough: trough = p
                        if p >= trough * (1 + winner["exit_pct"]):
                            exit_px = trough * (1 + winner["exit_pct"])
                if exit_px is None and i - entry_i >= winner["time_exit_bars"]:
                    exit_px = p
                if exit_px is not None:
                    ret = (exit_px - entry_px) / entry_px if side == 1 else (entry_px - exit_px) / entry_px
                    trades.append((sub.timestamp.iloc[signal_i], sub.timestamp.iloc[entry_i], entry_px, sub.timestamp.iloc[i], exit_px, ret))
                    in_pos = False
                    continue
            if not in_pos and i + 1 < len(sub):
                want_long = bool(long_gate[i])
                want_short = bool(short_gate[i])
                if want_long or want_short:
                    side = 1 if want_long else -1
                    signal_i = i
                    entry_i = i + 1
                    entry_px = float(sub.open.iloc[entry_i])
                    peak = trough = entry_px
                    in_pos = True
        if in_pos:
            exit_px = float(sub.close.iloc[-1])
            ret = (exit_px - entry_px) / entry_px if side == 1 else (entry_px - exit_px) / entry_px
            trades.append((None, sub.timestamp.iloc[entry_i], entry_px, sub.timestamp.iloc[-1], exit_px, ret))
        print(window_name)
        for signal_ts, entry_ts, entry_px, exit_ts, exit_px, ret in trades[:limit]:
            signal_text = signal_ts.isoformat() if signal_ts is not None else "NONE"
            print(f"{signal_text} {entry_ts.isoformat()} {entry_px:.8f} {exit_ts.isoformat()} {exit_px:.8f} {ret * 100:.6f}%")


def main():
    wins = base.windows()
    data = {}
    for asset in base.ASSETS:
        frame = pd.read_csv(base.BAR_DIR / f"{asset.replace('/','_')}_5m.csv", parse_dates=["timestamp"])
        frame.timestamp = pd.to_datetime(frame.timestamp, utc=True)
        data[asset] = {rule:resample(frame, rule) for rule in ("15min", "1h")}
    # All listed grid dimensions, with trailing alternatives replacing take-profit.
    exits = [("take_profit",v) for v in (0.02,0.04,0.06)] + [("trailing_stop",v) for v in (0.015,0.02,0.03)]
    configs=[]
    for resolution,family,session,direction,stop,exitv,time_exit in itertools.product(("15min","1h"),("trend","mean_reversion"),SESSIONS,("long_only","short_only","both"),(0.01,0.02,0.03),exits,(12,48,96)):
        ek, ep = exitv
        base_cfg={"resolution":resolution,"family":family,"session_utc":f"{session[0]:02d}-{session[1]:02d}","direction":direction,"stop_pct":stop,"exit_kind":ek,"exit_pct":ep,"time_exit_bars":time_exit}
        if family == "trend":
            for pair,adx,rvol in itertools.product(((9,21),(12,26),(10,40)),(20.,25.,30.),(1.0,1.3)):
                configs.append({**base_cfg,"ema_pair":list(pair),"adx_floor":adx,"rvol_floor":rvol})
        else: configs.append({**base_cfg,"ema_pair":None,"adx_floor":None,"rvol_floor":None})
    print(f"round2 evaluating {len(configs)} configs", flush=True)
    prepared={}
    ind_cache={}
    for asset in base.ASSETS:
        for rule, frame in data[asset].items():
            for w in wins:
                m=(frame.timestamp>=pd.Timestamp(w["start"]))&(frame.timestamp<pd.Timestamp(w["end"]))
                ix=np.flatnonzero(m.to_numpy())
                if not ix.size:
                    prepared[(asset,rule,w["name"])]=None
                    continue
                s,e=ix[0],ix[-1]+1
                sub=frame.iloc[s:e].reset_index(drop=True)
                prepared[(asset,rule,w["name"])]=(sub,)
                ind_cache[(asset,rule,w["name"])]=base.indicators(sub)
    gate_cache={}
    scored=[]
    for ci,cfg in enumerate(configs):
        asset_pnl=[]; total=0.
        for asset in base.ASSETS:
            for w in wins:
                key=(asset,cfg["resolution"],w["name"],cfg["family"],tuple(cfg["ema_pair"]) if cfg["ema_pair"] else None,cfg["adx_floor"],cfg["rvol_floor"],cfg["session_utc"])
                item=prepared[(asset,cfg["resolution"],w["name"])]
                pnl=0.
                if item is not None:
                    frame=item[0]
                    if key not in gate_cache:
                        gate_cache[key]=make_gates(frame,cfg["family"],tuple(cfg["ema_pair"]) if cfg["ema_pair"] else (9,21),cfg["adx_floor"] or 0.,cfg["rvol_floor"] or 0.,tuple(map(int,cfg["session_utc"].split('-'))),ind_cache[(asset,cfg["resolution"],w["name"])])
                    lg,sg=gate_cache[key]
                    if frame.shape[0]>WARMUP+1:
                        pnl=float(simulate_gate(frame.close.to_numpy(float),frame.open.to_numpy(float),lg,sg,cfg["stop_pct"],0 if cfg["exit_kind"]=="take_profit" else 1,cfg["exit_pct"],cfg["time_exit_bars"],DIRECTIONS[cfg["direction"]]))
                asset_pnl.append((w["name"],asset,pnl)); total += pnl
        by_window={w:sum(p for wn,a,p in asset_pnl if wn==w) for w in [x["name"] for x in wins]}
        profitable=sum(v>0 for v in by_window.values())
        scored.append((profitable,total,cfg,asset_pnl,by_window))
        if (ci+1)%500==0: print(f"  {ci+1}/{len(configs)}",flush=True)
    scored.sort(key=lambda x:(-x[0],-x[1]))
    family_best={}
    for family in ("trend","mean_reversion"):
        family_best[family]=max((x for x in scored if x[2]["family"]==family), key=lambda x:(x[0],x[1]))
    top=[]
    for rank,(prof,total,cfg,ap,bw) in enumerate(scored[:5],1):
        top.append({"rank":rank,"profitable_windows":prof,"total_windows":len(wins),"total_net_pnl":round(total,8),"parameters":cfg,"strategy":contract(cfg),"per_window_pnl":[{"window":w,"net_pnl":round(v,8)} for w,v in bw.items()],"per_asset_window_pnl":[{"window":w,"asset":a,"net_pnl":round(p,8)} for w,a,p in ap]})
    overall=top[0]
    family_summary={}
    for family,(prof,total,cfg,ap,bw) in family_best.items():
        family_summary[family]={"profitable_windows":prof,"total_windows":len(wins),"total_net_pnl":round(total,8),"parameters":cfg,"strategy":contract(cfg),"per_window_pnl":[{"window":w,"net_pnl":round(v,8)} for w,v in bw.items()]}
    out=json.loads((OUT/"results.json").read_text())
    out["round2"]={"method":{"resolutions":["15min","1h"],"families":["trend","mean_reversion"],"sessions_utc":[f"{a:02d}-{b:02d}" for a,b in SESSIONS],"strategy_level_scoring":True,"grid_config_count":len(configs)},"coverage":{"profitable_windows":overall["profitable_windows"],"total_windows":len(wins)},"best_by_family":family_summary,"top_configs":top,"best_config":overall["parameters"],"best_strategy":overall["strategy"]}
    (OUT/"results.json").write_text(json.dumps(out,indent=2)+"\n")
    report = (OUT/"REPORT.md").read_text()
    marker = "\n## Round two: lower frequency and mean reversion"
    if marker in report:
        report = report.split(marker, 1)[0].rstrip() + "\n"
    report += "\n## Round two: lower frequency and mean reversion\n\n"
    report += f"Round two evaluated {len(configs)} deterministic configurations at 15-minute and 1-hour resolution. It tested the original trend gate and a Bollinger(20, 2) plus RSI(14) mean-reversion gate, each in four UTC six-hour entry sessions. Results are scored at strategy level: the three asset PnLs are summed per window, then a window counts profitable when that sum is positive.\n\n"
    report += f"Best round-two result: **{overall['profitable_windows']}/{len(wins)} windows** profitable, total net PnL **${overall['total_net_pnl']:.2f}**.\n\n"
    for family, summary in family_summary.items():
        report += f"- Best `{family}`: **{summary['profitable_windows']}/{len(wins)}** windows, total net PnL **${summary['total_net_pnl']:.2f}**. Parameters: `{json.dumps(summary['parameters'], separators=(',', ':'))}`\n"
    report += "\n```json\n"+json.dumps(overall["parameters"],indent=2)+"\n```\n\n"
    report += "The full round-two top-five and per-asset-window table is in `results.json` under `round2`; `best_by_family` records the strongest configuration for each family.\n"
    (OUT/"REPORT.md").write_text(report)
    print(f"round2 best {overall['profitable_windows']}/{len(wins)} pnl={overall['total_net_pnl']:.2f}",flush=True)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dump-trades", action="store_true")
    args = parser.parse_args()
    dump_trades() if args.dump_trades else main()
