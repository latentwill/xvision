import json, sqlite3, statistics as st

DB = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/xvn.db"
db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
q = """
select r.agent_id, coalesce(a.name, r.agent_id), r.scenario_id,
       json_extract(s.body_json, '$.display_name'), json_extract(s.body_json, '$.granularity'),
       r.metrics_json, r.mode, r.started_at
from eval_runs r
left join agents a on a.agent_id = r.agent_id
left join scenarios s on s.id = r.scenario_id
where r.status='completed' and r.metrics_json is not null and s.body_json is not null
"""
rows = db.execute(q).fetchall()
print("total rows:", len(rows))
agg = {}
for agent_id, name, scen_id, disp, gran, mj, mode, ts in rows:
    try:
        m = json.loads(mj)
    except Exception:
        continue
    if not isinstance(m, dict) or "total_return_pct" not in m:
        continue
    b = m.get("baselines") or {}
    bh = (b.get("buy_hold") or {}).get("return_pct")
    tr = (b.get("simple_trend") or {}).get("return_pct")
    a = agg.setdefault(agent_id, {"name": name, "runs": [], "scen": set(), "gran": set(), "modes": set()})
    a["runs"].append({"ret": m["total_return_pct"], "sharpe": m.get("sharpe"), "dd": m.get("max_drawdown_pct"),
                      "wr": m.get("win_rate"), "bh": bh, "tr": tr, "ntr": m.get("n_trades"),
                      "scen": disp or scen_id, "gran": gran, "mode": mode})
    a["scen"].add(scen_id); a["gran"].add(gran); a["modes"].add(mode)


def summarize(a):
    rs = [r for r in a["runs"] if r["ret"] is not None]
    rets = [r["ret"] for r in rs]
    sh = [r["sharpe"] for r in rs if r["sharpe"] is not None]
    al_bh = [r["ret"] - r["bh"] for r in rs if r["bh"] is not None]
    al_tr = [r["ret"] - r["tr"] for r in rs if r["tr"] is not None]
    return {
        "name": a["name"], "n": len(rets),
        "avg_ret": st.mean(rets), "med_ret": st.median(rets),
        "win": sum(1 for x in rets if x > 0) / len(rets),
        "avg_sharpe": st.mean(sh) if sh else None,
        "alpha_bh": st.mean(al_bh) if al_bh else None,
        "alpha_tr": st.mean(al_tr) if al_tr else None,
        "avg_dd": st.mean([r["dd"] for r in rs if r["dd"] is not None]),
        "avg_wr": st.mean([r["wr"] for r in rs if r["wr"] is not None]),
        "avg_ntrades": st.mean([r["ntr"] or 0 for r in rs]),
        "scen_n": len(a["scen"]), "grans": sorted(str(g) for g in a["gran"]),
        "modes": sorted(a["modes"]),
    }


cands = [(aid, summarize(a)) for aid, a in agg.items() if len(a["runs"]) >= 8]
cands.sort(key=lambda t: t[1]["med_ret"], reverse=True)
hdr = ("agent", "name", "n", "avgRet", "medRet", "win%", "shrp", "aBH", "aTR", "dd%", "wr%")
print("%d agents with >=8 completed scored runs\n" % len(cands))
print("%-36s %-28s %4s %8s %8s %5s %6s %7s %7s %6s %4s  grans" % hdr)
for aid, s in cands[:20]:
    nm = (s["name"] or "")[:27]
    print("%-36s %-28s %4d %8.2f %8.2f %5.0f %6.2f %7.2f %7.2f %6.2f %4.0f  %s" % (
        aid[:35], nm, s["n"], s["avg_ret"], s["med_ret"], s["win"] * 100,
        s["avg_sharpe"] or 0, s["alpha_bh"] or 0, s["alpha_tr"] or 0,
        s["avg_dd"] or 0, (s["avg_wr"] or 0) * 100, ",".join(s["grans"])))

json.dump([(aid, s) for aid, s in cands], open("/tmp/lb.json", "w"), default=str)
